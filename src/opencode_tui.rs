use crate::executor::load_executors;
use crate::interactive::{
    build_interactive_home_with_options, render_interactive_status_for_store,
    route_interactive_input_with_context, InteractiveHomeOptions, InteractiveHomeReport,
};
use crate::storage::{ForgeStore, GlobalEventWrite};
use anyhow::Result;
use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyCode, KeyEventKind, KeyModifiers,
};
use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor};
use crossterm::terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{execute, queue};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::io::{stdout, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

const FORGE_TUI_SCHEMA_VERSION: &str = "forge.tui.opencode_orchestrator.v1";
const FORGE_TUI_ORCHESTRATOR_SCHEMA_VERSION: &str = "forge.tui.orchestrator.v1";
const FORGE_CHAT_SESSION_SCHEMA_VERSION: &str = "forge.tui.chat_session.v1";
const MAX_ACTIVITY_LINES: usize = 80;
const TUI_IDLE_SHELL_HANDOFF_TIMEOUT: Duration = Duration::from_secs(45);
const CHAT_SESSION_DIR_NAME: &str = ".forge/chat-sessions";
const CHAT_SESSION_LATEST_FILENAME: &str = "latest.json";

#[derive(Debug, Clone, Serialize)]
pub struct ForgeTuiReport {
    pub schema_version: String,
    pub status: String,
    pub layout: String,
    pub orchestrator: ForgeTuiOrchestrator,
    pub renderer_strategy: ForgeTuiRendererStrategy,
    pub prompt: ForgeTuiPrompt,
    pub shell: ForgeTuiShell,
    pub status_bar: ForgeTuiStatusBar,
    pub visualizations: Vec<ForgeTuiVisualization>,
    pub capabilities: Vec<ForgeTuiCapability>,
    pub quick_commands: Vec<String>,
    pub agent_suggestions: Vec<String>,
    pub file_suggestions: Vec<String>,
    pub workflow_suggestions: Vec<String>,
    pub context_suggestions: Vec<String>,
    pub session_tabs: Vec<String>,
    pub benchmark_snapshot: ForgeTuiBenchmarkSnapshot,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForgeTuiOrchestrator {
    pub schema_version: String,
    pub default_interaction: String,
    pub decision_policy: String,
    pub plan_mode: String,
    pub build_mode: String,
    pub agent_model: String,
    pub node_agent_routing: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForgeTuiRendererStrategy {
    pub schema_version: String,
    pub current_backend: String,
    pub target_backend: String,
    pub ecosystem_sources: Vec<String>,
    pub rust_native_candidates: Vec<String>,
    pub bridge_candidates: Vec<String>,
    pub component_system_candidates: Vec<String>,
    pub create_tui_template_family: String,
    pub next_step: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForgeTuiPrompt {
    pub placeholder: String,
    pub submit_hint: String,
    pub command_hint: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForgeTuiShell {
    pub enabled: bool,
    pub prefix: String,
    pub toggle: String,
    pub audit_event_kind: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForgeTuiStatusBar {
    pub workflows: usize,
    pub active_runs: usize,
    pub events: usize,
    pub addons: usize,
    pub capabilities: usize,
    pub ready_handoffs: usize,
    pub pending_approvals: usize,
    pub estimated_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForgeTuiVisualization {
    pub id: String,
    pub title: String,
    pub command: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForgeTuiCapability {
    pub id: String,
    pub title: String,
    pub command: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForgeTuiBenchmarkSnapshot {
    pub schema_version: String,
    pub summary: String,
    pub placement_lines: Vec<String>,
    pub executor_lines: Vec<String>,
    pub forge_line: String,
    pub live_notes: Vec<String>,
}

struct ForgeTuiRuntimeState {
    project_root: Option<PathBuf>,
    chat_session_code: String,
    chat_session_path: PathBuf,
    chat_session_created_at: String,
    input: String,
    shell_mode: bool,
    activity: Vec<String>,
    autocomplete: Vec<String>,
    autocomplete_index: usize,
    conversation_history: Vec<ConversationTurn>,
    last_interaction_at: Instant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConversationTurn {
    role: ConversationRole,
    text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum ConversationRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ForgeChatSessionRecord {
    schema_version: String,
    chat_session_code: String,
    project_root: String,
    created_at: String,
    updated_at: String,
    conversation_history: Vec<ConversationTurn>,
}

#[derive(Debug, Clone, Copy)]
struct PaneFrame {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
}

pub fn build_forge_tui(
    store: &ForgeStore,
    project_root: Option<PathBuf>,
) -> Result<ForgeTuiReport> {
    let project_root_for_files = project_root.clone();
    let home = build_interactive_home_with_options(store, InteractiveHomeOptions { project_root })?;
    let executors = load_executors(store)?;
    Ok(forge_tui_from_home(
        &home,
        &executors,
        project_root_for_files.as_deref(),
    ))
}

fn forge_tui_from_home(
    home: &InteractiveHomeReport,
    executors: &crate::executor::ExecutorSyncReport,
    project_root: Option<&Path>,
) -> ForgeTuiReport {
    let d = &home.dashboard;
    let status_bar = ForgeTuiStatusBar {
        workflows: d.task_board_panel.workflow_count,
        active_runs: d.active_runs,
        events: d.event_panel.total_event_count,
        addons: d.addon_capability_panel.enabled_addon_count,
        capabilities: d.addon_capability_panel.capability_count,
        ready_handoffs: d.task_board_panel.ready_handoffs,
        pending_approvals: d.pending_approvals,
        estimated_cost_usd: d.cost_panel.estimated_task_cost_total_usd,
    };

    let session_tabs = if d.shell_entrypoints.is_empty() {
        vec!["forge".to_string()]
    } else {
        d.shell_entrypoints.iter().take(4).cloned().collect()
    };

    let agent_suggestions = executors
        .brain_router
        .brains
        .iter()
        .take(12)
        .map(|brain| format!("@{} — {}", brain.id, brain.display_name))
        .collect::<Vec<_>>();

    let file_suggestions = collect_file_suggestions(project_root);

    let workflow_suggestions = d
        .workflow_sidebar_panel
        .groups
        .iter()
        .flat_map(|group| group.items.iter())
        .take(12)
        .map(|workflow| format!("~{} — {}", workflow.workflow_id, workflow.current_goal))
        .collect::<Vec<_>>();

    let context_suggestions = vec![
        format!(
            "&{}.{}.{}",
            d.operating_context_panel.organization_label,
            d.operating_context_panel.product_label,
            d.operating_context_panel.brand_label
        ),
        format!(
            "&{}.{}.{}",
            d.operating_context_panel.organization_label,
            d.operating_context_panel.brand_label,
            d.operating_context_panel.product_label
        ),
    ];
    let benchmark_snapshot = build_benchmark_snapshot(executors, &status_bar);

    let orchestrator = ForgeTuiOrchestrator {
        schema_version: FORGE_TUI_ORCHESTRATOR_SCHEMA_VERSION.to_string(),
        default_interaction: "conversation_with_forge_orchestrator".to_string(),
        decision_policy: "direct_answer_or_create_workflow".to_string(),
        plan_mode: "forge_workflow".to_string(),
        build_mode: "forge_workflow".to_string(),
        agent_model: "agents_and_subagents_are_workflows_or_nodes".to_string(),
        node_agent_routing: "per_node_agent_allowed".to_string(),
        summary:
            "normal input talks to the Forge orchestrator; slash commands configure the surface"
                .to_string(),
    };

    ForgeTuiReport {
        schema_version: FORGE_TUI_SCHEMA_VERSION.to_string(),
        status: "forge_tui_ready".to_string(),
        layout: "opencode_style_orchestrator_first_tui".to_string(),
        orchestrator,
        renderer_strategy: ForgeTuiRendererStrategy {
            schema_version: "forge.tui.renderer_strategy.v1".to_string(),
            current_backend: "rust_crossterm_fullscreen_fallback".to_string(),
            target_backend: "opentui_native_core_bridge_or_incremental_rust_port".to_string(),
            ecosystem_sources: vec![
                "anomalyco/opentui native Zig renderer and TypeScript bindings".to_string(),
                "msmps/create-tui OpenTUI starter templates".to_string(),
                "msmps/awesome-opentui ecosystem map and testing references".to_string(),
                "msmps/opentui-ui component, dialog, toast and styled-slot patterns".to_string(),
            ],
            rust_native_candidates: vec![
                "Forge state projection and orchestrator routing".to_string(),
                "Terminal lifecycle fallback for environments without Bun/Zig".to_string(),
                "Key command routing and shell command audit events".to_string(),
                "Workflow, agent, subagent and node-agent data contracts".to_string(),
                "Forge-owned component metadata, slots, states and variant contracts".to_string(),
            ],
            bridge_candidates: vec![
                "OpenTUI Zig/C ABI renderer and optimized buffer".to_string(),
                "Yoga-style flex layout and component tree reconciliation".to_string(),
                "Unicode grapheme width, text buffer, editor, diff and markdown renderables"
                    .to_string(),
                "Mouse hit grid, split-footer scrollback, remote feed and frame statistics"
                    .to_string(),
                "create-tui Core/React/Solid bootstrap templates for generated renderer surfaces"
                    .to_string(),
            ],
            component_system_candidates: vec![
                "opentui-ui-style slot metadata for panels, badges, prompts and controls"
                    .to_string(),
                "state selectors for focused, selected, disabled, loading and errored UI states"
                    .to_string(),
                "dialog manager pattern for confirmations, handoffs and approvals".to_string(),
                "toast pattern for operation feedback, shell events and workflow notifications"
                    .to_string(),
            ],
            create_tui_template_family:
                "core template for Forge-owned runtime; React/Solid only for external Addon renderer prototypes"
                    .to_string(),
            next_step:
                "prototype a Forge TUI renderer adapter that can choose crossterm fallback or OpenTUI bridge"
                    .to_string(),
        },
        prompt: ForgeTuiPrompt {
            placeholder: "Talk to Forge; use /, @, ~, & for autocomplete".to_string(),
            submit_hint: "Enter sends the message; Forge answers directly or opens a workflow"
                .to_string(),
            command_hint: "!<cmd> runs local shell; ! toggles shell mode".to_string(),
        },
        shell: ForgeTuiShell {
            enabled: true,
            prefix: "!".to_string(),
            toggle: "!".to_string(),
            audit_event_kind: "forge_tui_shell_command".to_string(),
        },
        visualizations: vec![
            visualization(
                "workflows",
                "Workflows",
                "/workflows",
                format!(
                    "{} workflows, {} active runs, {} ready handoffs",
                    status_bar.workflows, status_bar.active_runs, status_bar.ready_handoffs
                ),
            ),
            visualization(
                "agents",
                "Agents",
                "/agents",
                format!(
                    "orchestrator plus {} visible execution session(s)",
                    session_tabs.len()
                ),
            ),
            visualization(
                "subagents",
                "Subagents",
                "/subagents",
                "subagents are Forge workflows, child workflows or DAG nodes".to_string(),
            ),
            visualization(
                "node_agents",
                "Node agents",
                "/nodes",
                "each workflow node can carry its own agent/brain routing".to_string(),
            ),
        ],
        capabilities: vec![
            capability(
                "orchestrator",
                "Orchestrator",
                "/orchestrator",
                "decides direct answer or workflow".to_string(),
            ),
            capability(
                "plan_workflows",
                "Plan workflows",
                "/plan",
                "plan is a Forge workflow".to_string(),
            ),
            capability(
                "build_workflows",
                "Build workflows",
                "/build",
                "build is a Forge workflow".to_string(),
            ),
            capability(
                "workflows",
                "Workflows",
                "/workflows",
                format!("{} total", status_bar.workflows),
            ),
            capability(
                "agents",
                "Agents",
                "/agents",
                "agents are workflows or nodes".to_string(),
            ),
            capability(
                "subagents",
                "Subagents",
                "/subagents",
                "subagents are child workflows or nodes".to_string(),
            ),
            capability(
                "node_agents",
                "Node agents",
                "/nodes",
                "per-node agent allowed".to_string(),
            ),
            capability(
                "events",
                "Events",
                "/events",
                format!("{} recorded", status_bar.events),
            ),
            capability(
                "addons",
                "Addons",
                "/addons",
                format!("{} enabled", status_bar.addons),
            ),
            capability(
                "costs",
                "Costs",
                "/costs",
                format!("estimated ${:.4}", status_bar.estimated_cost_usd),
            ),
            capability(
                "handoffs",
                "Handoffs",
                "/handoffs",
                format!("{} ready", status_bar.ready_handoffs),
            ),
            capability(
                "approvals",
                "Approvals",
                "/approvals",
                format!("{} pending", status_bar.pending_approvals),
            ),
            capability(
                "core_boundary",
                "Core boundary",
                "/boundary",
                d.core_boundary_panel.status.clone(),
            ),
            capability(
                "shells",
                "Shells",
                "/shells",
                format!("{} visible", session_tabs.len()),
            ),
            capability(
                "command_palette",
                "Command Palette",
                "/commands",
                format!("{} actions", d.command_palette_panel.entry_count),
            ),
            capability(
                "benchmark",
                "Benchmark",
                "/benchmark",
                "compare local CLI and renderer ideas".to_string(),
            ),
        ],
        quick_commands: vec![
            "/orchestrator".to_string(),
            "/workflows".to_string(),
            "/agents".to_string(),
            "/subagents".to_string(),
            "/nodes".to_string(),
            "/events".to_string(),
            "/addons".to_string(),
            "/costs".to_string(),
            "/approvals".to_string(),
            "/config".to_string(),
            "/boundary".to_string(),
            "/shells".to_string(),
            "/benchmark".to_string(),
            "/commands".to_string(),
            "/help".to_string(),
        ],
        agent_suggestions,
        file_suggestions,
        workflow_suggestions,
        context_suggestions,
        session_tabs,
        benchmark_snapshot,
        notes: vec![
            "OpenCode-inspired terminal surface; Forge owns orchestration and state.".to_string(),
            "Legacy detailed panels remain under `forge interactive ...`.".to_string(),
        ],
        status_bar,
    }
}

fn capability(id: &str, title: &str, command: &str, summary: String) -> ForgeTuiCapability {
    ForgeTuiCapability {
        id: id.to_string(),
        title: title.to_string(),
        command: command.to_string(),
        summary,
    }
}

fn visualization(id: &str, title: &str, command: &str, summary: String) -> ForgeTuiVisualization {
    ForgeTuiVisualization {
        id: id.to_string(),
        title: title.to_string(),
        command: command.to_string(),
        summary,
    }
}

fn build_benchmark_snapshot(
    executors: &crate::executor::ExecutorSyncReport,
    status_bar: &ForgeTuiStatusBar,
) -> ForgeTuiBenchmarkSnapshot {
    let wanted = ["codex", "gemini", "opencode", "claude"];
    let mut executor_lines = Vec::new();
    for id in wanted {
        let executor = executors
            .executors
            .iter()
            .find(|executor| executor.id == id);
        let label = match id {
            "codex" => "Codex",
            "gemini" => "Gemini",
            "opencode" => "OpenCode",
            "claude" => "Claude",
            _ => id,
        };
        let _command = executor
            .map(|executor| executor.command.clone())
            .unwrap_or_else(|| id.to_string());
        let command_path = find_command_on_path(_command.as_str());
        let benchmark = command_path
            .as_deref()
            .and_then(|path| probe_cli_benchmark(label, path));

        if let Some(benchmark) = benchmark {
            executor_lines.push(format!(
                "{}: {}, {}",
                label, benchmark.version, benchmark.features
            ));
        } else {
            executor_lines.push(format!("{}: unavailable", label));
        }
    }

    let forge_line = format!(
        "Forge: workflows={}, active_runs={}, handoffs={}, approvals={}, costs=${:.4}",
        status_bar.workflows,
        status_bar.active_runs,
        status_bar.ready_handoffs,
        status_bar.pending_approvals,
        status_bar.estimated_cost_usd
    );

    ForgeTuiBenchmarkSnapshot {
        schema_version: "forge.tui.benchmark_snapshot.v1".to_string(),
        summary:
            "live version and feature comparison of local executors plus Forge orchestration surface"
                .to_string(),
        placement_lines: vec![
            "Placement: Core = Codex, Gemini, OpenCode.".to_string(),
            "Placement: Addon-first = OpenClaw, Hermes, Open Design, Penpot, n8n.".to_string(),
        ],
        executor_lines,
        forge_line,
        live_notes: vec![
            "OpenClaw benchmark: async, multi-channel operator surface with durable handoff state.".to_string(),
            "Hermes benchmark: file-first memory with semantic retrieval and scope-aware promotion.".to_string(),
            "Codex is strongest when Forge wants direct execution, review and apply flows.".to_string(),
            "Gemini is the closest shell-first interactive reference.".to_string(),
            "OpenCode is the closest project-first TUI reference.".to_string(),
            "Claude is reported even when not installed, so the gap stays visible.".to_string(),
        ],
    }
}

struct CliBenchmarkProbe {
    version: String,
    features: String,
}

fn probe_cli_benchmark(label: &str, path: &Path) -> Option<CliBenchmarkProbe> {
    let version = run_cli_probe(path, &["--version"], Duration::from_secs(2)).ok()?;
    let help = run_cli_probe(path, &["--help"], Duration::from_secs(2)).ok()?;
    if version.status.map_or(false, |status| !status.success())
        && help.status.map_or(false, |status| !status.success())
    {
        return None;
    }

    let version = first_nonempty_line(&version.stdout)
        .or_else(|| first_nonempty_line(&version.stderr))
        .unwrap_or_else(|| "version unavailable".to_string());
    let features = summarize_cli_features(label, &help.stdout, &help.stderr);
    Some(CliBenchmarkProbe { version, features })
}

fn summarize_cli_features(label: &str, stdout: &str, stderr: &str) -> String {
    let help = format!("{stdout}\n{stderr}").to_ascii_lowercase();
    let features = match label {
        "Codex" => vec![
            ("exec", "exec"),
            ("review", "review"),
            ("apply", "apply"),
            ("sandbox", "sandbox"),
            ("resume", "resume"),
            ("fork", "fork"),
            ("mcp", "mcp"),
            ("remote-control", "remote-control"),
        ],
        "Gemini" => vec![
            ("interactive", "interactive"),
            ("resume", "resume"),
            ("session", "session"),
            ("approval-mode", "approval-mode"),
            ("extensions", "extensions"),
            ("skills", "skills"),
            ("hooks", "hooks"),
            ("screen-reader", "screen-reader"),
        ],
        "OpenCode" => vec![
            ("tui", "tui"),
            ("providers", "providers"),
            ("agent", "agent"),
            ("models", "models"),
            ("stats", "stats"),
            ("web", "web"),
            ("session", "session"),
            ("mcp", "mcp"),
        ],
        _ => Vec::new(),
    };

    let selected = features
        .iter()
        .filter_map(|(needle, label)| help.contains(needle).then_some(*label))
        .take(4)
        .collect::<Vec<_>>();
    if selected.is_empty() {
        "no notable features detected".to_string()
    } else {
        selected.join("/")
    }
}

fn first_nonempty_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.to_string())
}

fn find_command_on_path(command: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path_var) {
        let candidate = directory.join(command);
        if is_executable_path(&candidate) {
            return Some(candidate);
        }
    }
    None
}

struct CliProbeOutput {
    status: Option<std::process::ExitStatus>,
    stdout: String,
    stderr: String,
}

fn run_cli_probe(path: &Path, args: &[&str], timeout: Duration) -> Result<CliProbeOutput> {
    let mut child = Command::new(path)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let start = Instant::now();
    let mut timed_out = false;

    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if start.elapsed() >= timeout {
            timed_out = true;
            let _ = child.kill();
            break child.wait().ok();
        }
        thread::sleep(Duration::from_millis(25));
    };

    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        let _ = pipe.read_to_string(&mut stdout);
    }
    if let Some(mut pipe) = child.stderr.take() {
        let _ = pipe.read_to_string(&mut stderr);
    }
    if timed_out && status.is_none() {
        return Err(anyhow::anyhow!("probe timed out"));
    }

    Ok(CliProbeOutput {
        status,
        stdout,
        stderr,
    })
}

fn is_executable_path(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        return path
            .metadata()
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false);
    }
    #[cfg(not(unix))]
    {
        true
    }
}

pub fn render_forge_tui(report: &ForgeTuiReport) -> String {
    format!(
        "forge\n\
         Forge chat TUI - OpenCode-style\n\
         Prompt: {placeholder}\n\
         History and input stay on the live terminal surface.\n\
         forge> ",
        placeholder = report.prompt.placeholder,
    )
}

pub fn run_forge_tui(store_path: &Path, project_root: Option<PathBuf>) -> Result<i32> {
    let store = ForgeStore::open(store_path)?;
    let report = build_forge_tui(&store, project_root.clone())?;

    if !std::io::stdin().is_terminal() {
        println!("{}", render_forge_tui(&report));
        return Ok(0);
    }

    run_fullscreen_tui(&store, project_root, report)
}

fn run_fullscreen_tui(
    store: &ForgeStore,
    project_root: Option<PathBuf>,
    mut report: ForgeTuiReport,
) -> Result<i32> {
    let chat_session = create_chat_session_state(project_root.as_deref())?;
    let mut stdout = stdout();
    terminal::enable_raw_mode()?;
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableBracketedPaste,
        EnableMouseCapture,
        Hide
    )?;
    let _guard = ForgeTuiTerminalGuard;
    let chat_session_code = chat_session.chat_session_code.clone();

    let mut state = ForgeTuiRuntimeState {
        project_root: project_root.clone(),
        chat_session_code,
        chat_session_path: chat_session_path(project_root.as_deref(), &chat_session.chat_session_code),
        chat_session_created_at: chat_session.created_at,
        input: String::new(),
        shell_mode: false,
        activity: vec!["Forge ready.".to_string()],
        autocomplete: Vec::new(),
        autocomplete_index: 0,
        conversation_history: chat_session.conversation_history,
        last_interaction_at: Instant::now(),
    };

    refresh_tui_autocomplete(&mut state, &report);
    persist_chat_session(&state)?;
    render_fullscreen(&mut stdout, &report, &state)?;
    loop {
        if !event::poll(Duration::from_millis(250))? {
            if !maybe_handoff_to_shell_due_to_idle(
                &mut state,
                Instant::now(),
                TUI_IDLE_SHELL_HANDOFF_TIMEOUT,
            ) {
                continue;
            }
        } else {
            match event::read()? {
                Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        break;
                    }
                    KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        break;
                    }
                    KeyCode::Char('j' | 'm') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        touch_interaction(&mut state);
                        let input = state.input.trim().to_string();
                        state.input.clear();
                        if handle_tui_submit(
                            store,
                            &mut report,
                            project_root.clone(),
                            &mut state,
                            &input,
                        )? {
                            break;
                        }
                    }
                    KeyCode::Char(ch) => {
                        touch_interaction(&mut state);
                        state.input.push(ch);
                    }
                    KeyCode::Backspace => {
                        touch_interaction(&mut state);
                        state.input.pop();
                    }
                    KeyCode::Up => {
                        touch_interaction(&mut state);
                        if state.shell_mode {
                            continue;
                        }
                        if !state.autocomplete.is_empty() {
                            move_autocomplete_selection(&mut state, -1);
                        } else if is_counter_field(&state.input) {
                            adjust_counter_input(&mut state.input, 1);
                        }
                    }
                    KeyCode::Down => {
                        touch_interaction(&mut state);
                        if state.shell_mode {
                            continue;
                        }
                        if !state.autocomplete.is_empty() {
                            move_autocomplete_selection(&mut state, 1);
                        } else if is_counter_field(&state.input) {
                            adjust_counter_input(&mut state.input, -1);
                        }
                    }
                    KeyCode::Left | KeyCode::Right => {
                        touch_interaction(&mut state);
                    }
                    KeyCode::Enter => {
                        touch_interaction(&mut state);
                        let input = state.input.trim().to_string();
                        state.input.clear();
                        if handle_tui_submit(
                            store,
                            &mut report,
                            project_root.clone(),
                            &mut state,
                            &input,
                        )? {
                            break;
                        }
                    }
                    KeyCode::Esc => {
                        touch_interaction(&mut state);
                        if state.shell_mode {
                            set_shell_mode(&mut state, false, "Esc");
                        } else {
                            state.input.clear();
                            state.autocomplete.clear();
                            state.autocomplete_index = 0;
                        }
                    }
                    KeyCode::Tab => {
                        touch_interaction(&mut state);
                        if state.shell_mode {
                            set_shell_mode(&mut state, false, "Tab");
                        } else if !state.autocomplete.is_empty() {
                            accept_autocomplete_selection(&mut state);
                        }
                    }
                    _ => {}
                },
                Event::Paste(text) => {
                    touch_interaction(&mut state);
                    state.input.push_str(&text)
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
        refresh_tui_autocomplete(&mut state, &report);
        render_fullscreen(&mut stdout, &report, &state)?;
    }

    drop(_guard);
    println!("goodbye");
    println!("Chat code: {}", state.chat_session_code);
    Ok(0)
}

struct ForgeTuiTerminalGuard;

impl Drop for ForgeTuiTerminalGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let mut stdout = stdout();
        let _ = execute!(
            stdout,
            Show,
            DisableMouseCapture,
            DisableBracketedPaste,
            ResetColor,
            LeaveAlternateScreen
        );
    }
}

fn handle_tui_submit(
    store: &ForgeStore,
    report: &mut ForgeTuiReport,
    project_root: Option<PathBuf>,
    state: &mut ForgeTuiRuntimeState,
    input: &str,
) -> Result<bool> {
    touch_interaction(state);
    if input.is_empty() {
        return Ok(false);
    }
    if input.starts_with("/resume") {
        let lines = resume_chat_session(store, project_root.as_deref(), state, input)?;
        for line in lines {
            push_activity(state, line);
        }
        persist_chat_session(state)?;
        *report = build_forge_tui(store, project_root)?;
        return Ok(false);
    }
    record_conversation_turn(state, ConversationRole::User, input);
    push_activity(state, format!("{} {}", prompt_label(state), input));
    state.autocomplete.clear();
    state.autocomplete_index = 0;

    if matches!(input, "q" | "quit" | "exit" | "/quit" | "/exit") {
        push_activity(state, "goodbye".to_string());
        record_conversation_turn(state, ConversationRole::Assistant, "goodbye");
        return Ok(true);
    }

    if input == "!" {
        let enable_shell = !state.shell_mode;
        set_shell_mode(state, enable_shell, "!");
        return Ok(false);
    }

    if state.shell_mode {
        let shell_lines = dispatch_shell_command(store, input)?;
        for line in &shell_lines {
            push_activity(state, line.clone());
        }
        if let Some(last) = shell_lines.last() {
            record_conversation_turn(state, ConversationRole::Assistant, last);
        }
        *report = build_forge_tui(store, project_root)?;
        return Ok(false);
    }

    if let Some(command) = input.strip_prefix('!').map(str::trim) {
        if command.is_empty() {
            set_shell_mode(state, true, "!");
        } else {
            for line in dispatch_shell_command(store, command)? {
                push_activity(state, line);
            }
            *report = build_forge_tui(store, project_root)?;
        }
        return Ok(false);
    }

    if input == "/status" {
        for line in render_interactive_status_for_store(store)?.lines() {
            push_activity(state, line.to_string());
        }
        *report = build_forge_tui(store, project_root)?;
        return Ok(false);
    }

    if let Some(lines) = dispatch_forge_tui_command(report, input) {
        for line in &lines {
            push_activity(state, line.clone());
        }
        if let Some(last) = lines.last() {
            record_conversation_turn(state, ConversationRole::Assistant, last);
        }
        return Ok(false);
    }

    let conversation_context = conversation_context_for_brain(&state.conversation_history);
    let route =
        route_interactive_input_with_context(store, input, "forge_tui", &conversation_context)?;
    if let Some(answer) = route.answer {
        push_activity(state, answer.clone());
        record_conversation_turn(state, ConversationRole::Assistant, &answer);
    }
    if route.workflow_created {
        if let Some(run_id) = route.run_id {
            push_activity(state, format!("Run ID: {run_id}"));
        }
        if let Some(workflow_id) = route.workflow_id {
            push_activity(state, format!("Workflow ID: {workflow_id}"));
        }
        record_conversation_turn(
            state,
            ConversationRole::Assistant,
            route.routing_explanation.as_str(),
        );
    }
    persist_chat_session(state)?;
    *report = build_forge_tui(store, project_root)?;
    Ok(false)
}

fn record_conversation_turn(state: &mut ForgeTuiRuntimeState, role: ConversationRole, text: &str) {
    state.conversation_history.push(ConversationTurn {
        role,
        text: text.to_string(),
    });
    if state.conversation_history.len() > 64 {
        let remove_count = state.conversation_history.len() - 64;
        state.conversation_history.drain(0..remove_count);
    }
}

fn create_chat_session_state(project_root: Option<&Path>) -> Result<ForgeChatSessionRecord> {
    let code = format!("chat_{}", Uuid::new_v4().to_string().replace('-', ""));
    let now = chrono::Utc::now().to_rfc3339();
    let record = ForgeChatSessionRecord {
        schema_version: FORGE_CHAT_SESSION_SCHEMA_VERSION.to_string(),
        chat_session_code: code,
        project_root: project_root
            .map(|root| root.display().to_string())
            .unwrap_or_else(|| {
                std::env::current_dir()
                    .map_or_else(|_| ".".to_string(), |cwd| cwd.display().to_string())
            }),
        created_at: now.clone(),
        updated_at: now,
        conversation_history: Vec::new(),
    };
    persist_chat_session_record(project_root, &record)?;
    Ok(record)
}

fn chat_session_directory(project_root: Option<&Path>) -> PathBuf {
    project_root
        .map(|root| root.join(CHAT_SESSION_DIR_NAME))
        .unwrap_or_else(|| {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(CHAT_SESSION_DIR_NAME)
        })
}

fn chat_session_path(project_root: Option<&Path>, code: &str) -> PathBuf {
    chat_session_directory(project_root).join(format!("{code}.json"))
}

fn latest_chat_session_path(project_root: Option<&Path>) -> PathBuf {
    chat_session_directory(project_root).join(CHAT_SESSION_LATEST_FILENAME)
}

fn persist_chat_session(state: &ForgeTuiRuntimeState) -> Result<()> {
    let record = ForgeChatSessionRecord {
        schema_version: FORGE_CHAT_SESSION_SCHEMA_VERSION.to_string(),
        chat_session_code: state.chat_session_code.clone(),
        project_root: state
            .project_root
            .as_ref()
            .map(|root| root.display().to_string())
            .unwrap_or_else(|| {
                std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .display()
                    .to_string()
            }),
        created_at: state.chat_session_created_at.clone(),
        updated_at: chrono::Utc::now().to_rfc3339(),
        conversation_history: state.conversation_history.clone(),
    };
    persist_chat_session_record(Some(state.chat_session_path.as_path()), &record)
}

fn persist_chat_session_record(
    project_root_or_path: Option<&Path>,
    record: &ForgeChatSessionRecord,
) -> Result<()> {
    let session_dir = match project_root_or_path {
        Some(path) if path.ends_with(CHAT_SESSION_LATEST_FILENAME) || path.extension().is_some() => {
            path.parent().unwrap_or(path).to_path_buf()
        }
        Some(path) => path.to_path_buf(),
        None => chat_session_directory(None),
    };
    fs::create_dir_all(&session_dir)?;
    let record_json = serde_json::to_string_pretty(record)?;
    let code_path = session_dir.join(format!("{}.json", record.chat_session_code));
    fs::write(&code_path, &record_json)?;
    let latest_path = session_dir.join(CHAT_SESSION_LATEST_FILENAME);
    fs::write(&latest_path, &record_json)?;
    Ok(())
}

#[allow(dead_code)]
fn load_chat_session_record(
    project_root: Option<&Path>,
    code: Option<&str>,
) -> Result<Option<ForgeChatSessionRecord>> {
    load_chat_session_record_with_exclusion(project_root, code, None)
}

fn load_chat_session_record_with_exclusion(
    project_root: Option<&Path>,
    code: Option<&str>,
    exclude_code: Option<&str>,
) -> Result<Option<ForgeChatSessionRecord>> {
    let session_dir = chat_session_directory(project_root);
    if let Some(code) = code.filter(|code| !code.trim().is_empty()) {
        let path = chat_session_path(project_root, code.trim());
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&path)?;
        let record: ForgeChatSessionRecord = serde_json::from_str(&content)?;
        return Ok(Some(record));
    }

    let latest_path = latest_chat_session_path(project_root);
    if latest_path.exists() {
        let content = fs::read_to_string(&latest_path)?;
        let record: ForgeChatSessionRecord = serde_json::from_str(&content)?;
        if exclude_code.is_none_or(|exclude| exclude != record.chat_session_code) {
            return Ok(Some(record));
        }
    }

    let mut candidates: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    for entry in fs::read_dir(&session_dir).into_iter().flatten().flatten() {
        let path = entry.path();
        let is_session_json = path.extension().and_then(|ext| ext.to_str()) == Some("json");
        let is_latest = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == CHAT_SESSION_LATEST_FILENAME);
        if !is_session_json || is_latest {
            continue;
        }
        if exclude_code.is_some_and(|exclude| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .is_some_and(|stem| stem == exclude)
        }) {
            continue;
        }
        if let Ok(metadata) = fs::metadata(&path) {
            if let Ok(modified) = metadata.modified() {
                candidates.push((modified, path));
            }
        }
    }
    candidates.sort_by(|left, right| right.0.cmp(&left.0));
    let Some((_, path)) = candidates.first() else {
        return Ok(None);
    };
    let content = fs::read_to_string(path)?;
    let record: ForgeChatSessionRecord = serde_json::from_str(&content)?;
    Ok(Some(record))
}

fn resume_chat_session(
    _store: &ForgeStore,
    project_root: Option<&Path>,
    state: &mut ForgeTuiRuntimeState,
    input: &str,
) -> Result<Vec<String>> {
    let code = input
        .strip_prefix("/resume")
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let record = load_chat_session_record_with_exclusion(project_root, code, Some(&state.chat_session_code))?;
    let record = match record {
        Some(record) => record,
        None => {
            let mut lines = vec!["No saved chat session found.".to_string()];
            if let Some(code) = code {
                lines.push(format!("Requested code not found: {code}"));
            } else {
                lines.push("Use /resume <code> after closing a prior chat.".to_string());
            }
            return Ok(lines);
        }
    };

    state.chat_session_code = record.chat_session_code.clone();
    state.chat_session_path = chat_session_path(project_root, &record.chat_session_code);
    state.chat_session_created_at = record.created_at.clone();
    state.conversation_history = record.conversation_history.clone();
    state.input.clear();
    state.autocomplete.clear();
    state.autocomplete_index = 0;
    state.shell_mode = false;

    let mut lines = vec![format!(
        "Resumed chat {} with {} turn(s).",
        record.chat_session_code,
        record.conversation_history.len()
    )];
    if let Some(last_user) = record
        .conversation_history
        .iter()
        .rev()
        .find(|turn| matches!(turn.role, ConversationRole::User))
    {
        lines.push(format!("Last user turn: {}", last_user.text));
    }
    lines.push("You can continue chatting or close with the same code.".to_string());
    Ok(lines)
}

fn conversation_context_for_brain(history: &[ConversationTurn]) -> Vec<String> {
    history
        .iter()
        .rev()
        .take(8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|turn| {
            let role = match turn.role {
                ConversationRole::User => "user",
                ConversationRole::Assistant => "assistant",
            };
            format!("{role}: {}", turn.text)
        })
        .collect()
}

fn dispatch_forge_tui_command(report: &ForgeTuiReport, input: &str) -> Option<Vec<String>> {
    match input {
        "/help" | "/commands" => Some(vec![
            "Commands: /status /orchestrator /workflows /agents /subagents /nodes /events"
                .to_string(),
            "Commands: /addons /costs /approvals /config /boundary /benchmark".to_string(),
            "Commands: /resume [code] /shells /commands /help".to_string(),
            "Shell: !<cmd> or ! to toggle shell mode".to_string(),
        ]),
        "/orchestrator" => Some(vec![
            "Orchestrator: default conversation".to_string(),
            "The orchestrator decides direct answer or workflow.".to_string(),
            "Plan and build are Forge workflows.".to_string(),
        ]),
        "/workflows" => Some(vec![format!(
            "Workflows: {} total; active runs {}",
            report.status_bar.workflows, report.status_bar.active_runs
        )]),
        "/agents" => Some(vec![
            "Agents: agents are Forge workflows or DAG nodes.".to_string(),
            "Each node can bind a specific agent/brain routing contract.".to_string(),
        ]),
        "/subagents" => Some(vec![
            "Subagents: subagents are child workflows, subflows or nodes.".to_string(),
            "They remain visible in the workflow graph instead of hidden model calls.".to_string(),
        ]),
        "/nodes" | "/node-agents" => Some(vec![
            "Node agents: per-node agent routing allowed.".to_string(),
            "Use forge workflow update-node-brain to mutate routing without stopping a run."
                .to_string(),
        ]),
        "/events" => Some(vec![format!(
            "Events: {} recorded",
            report.status_bar.events
        )]),
        "/addons" => Some(vec![format!(
            "Addons: {} enabled; capabilities {}",
            report.status_bar.addons, report.status_bar.capabilities
        )]),
        "/costs" => Some(vec![format!(
            "Costs: estimated ${:.4}",
            report.status_bar.estimated_cost_usd
        )]),
        "/handoffs" => Some(vec![format!(
            "Handoffs: {} ready",
            report.status_bar.ready_handoffs
        )]),
        "/approvals" => Some(vec![format!(
            "Approvals: {} pending",
            report.status_bar.pending_approvals
        )]),
        "/config" => Some(vec![
            "Configuration: use slash commands for views and behavior.".to_string(),
            "Normal text stays conversation with the Forge orchestrator.".to_string(),
        ]),
        "/boundary" | "/core-boundary" => report
            .capabilities
            .iter()
            .find(|capability| capability.id == "core_boundary")
            .map(|capability| vec![format!("Core boundary: {}", capability.summary)]),
        "/shells" => Some(vec![format!(
            "Shell sessions: {}",
            report.session_tabs.join(" | ")
        )]),
        "/benchmark" => Some(render_local_benchmark_lines(report)),
        _ => None,
    }
}

fn render_local_benchmark_lines(report: &ForgeTuiReport) -> Vec<String> {
    let mut lines = vec![format!("Benchmark: {}", report.benchmark_snapshot.summary)];
    lines.extend(report.benchmark_snapshot.placement_lines.clone());
    lines.extend(report.benchmark_snapshot.executor_lines.clone());
    lines.push(report.benchmark_snapshot.forge_line.clone());
    lines.extend(report.benchmark_snapshot.live_notes.iter().take(2).cloned());
    lines.push(
        "Forge additions: selector autocomplete, contextual forms, shell handoff, workflow visibility."
            .to_string(),
    );
    lines
}

fn dispatch_shell_command(store: &ForgeStore, command: &str) -> Result<Vec<String>> {
    let mut lines = vec![format!("Shell command: {command}")];
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let output = Command::new(shell).arg("-lc").arg(command).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    for line in stdout.lines() {
        if !line.trim().is_empty() {
            lines.push(line.to_string());
        }
    }
    for line in stderr.lines() {
        if !line.trim().is_empty() {
            lines.push(line.to_string());
        }
    }
    let exit_code = output.status.code().unwrap_or_default();
    lines.push(format!("Shell exit: {exit_code}"));
    record_shell_event(store, command, exit_code)?;
    Ok(lines)
}

fn record_shell_event(store: &ForgeStore, command: &str, exit_code: i32) -> Result<()> {
    let data = json!({
        "schema_version": "forge.tui.shell_command.v1",
        "command": command,
        "exit_code": exit_code,
    });
    let tenant = json!({});
    store.record_global_event(GlobalEventWrite {
        source: "forge.tui",
        source_id: "forge-tui",
        workflow_id: None,
        kind: "forge_tui_shell_command",
        origin: "forge_tui",
        status: if exit_code == 0 {
            "completed"
        } else {
            "failed"
        },
        data: &data,
        tenant_context: &tenant,
    })?;
    Ok(())
}

fn render_fullscreen(
    stdout: &mut std::io::Stdout,
    report: &ForgeTuiReport,
    state: &ForgeTuiRuntimeState,
) -> Result<()> {
    let (width, height) = terminal::size().unwrap_or((100, 32));
    let width = width.max(72);
    let height = height.max(24);
    queue!(stdout, MoveTo(0, 0), Clear(ClearType::All))?;

    let body_top = 0;
    let input_height = 6;
    let body_height = height.saturating_sub(body_top + input_height + 1);

    draw_box(
        stdout,
        PaneFrame {
            x: 0,
            y: body_top,
            width,
            height: body_height.max(3),
        },
        "HISTORY",
        &activity_lines(state, body_height.saturating_sub(2) as usize),
        Color::Yellow,
    )?;

    draw_input(
        stdout,
        report,
        state,
        width,
        height.saturating_sub(input_height),
    )?;
    stdout.flush()?;
    Ok(())
}

fn draw_input(
    stdout: &mut std::io::Stdout,
    report: &ForgeTuiReport,
    state: &ForgeTuiRuntimeState,
    width: u16,
    y: u16,
) -> Result<()> {
    draw_box(
        stdout,
        PaneFrame {
            x: 0,
            y,
            width,
            height: 6,
        },
        if state.shell_mode {
            "SHELL INPUT"
        } else {
            "CHAT INPUT"
        },
        &draw_input_lines(report, state),
        if state.shell_mode {
            Color::Yellow
        } else {
            Color::Cyan
        },
    )
}

fn draw_box(
    stdout: &mut std::io::Stdout,
    frame: PaneFrame,
    title: &str,
    lines: &[String],
    color: Color,
) -> Result<()> {
    let PaneFrame {
        x,
        y,
        width,
        height,
    } = frame;
    if width < 4 || height < 3 {
        return Ok(());
    }
    let inner = width.saturating_sub(2) as usize;
    queue!(
        stdout,
        SetForegroundColor(color),
        MoveTo(x, y),
        Print(format!("╭{}╮", "─".repeat(inner))),
        MoveTo(x + 2, y),
        SetAttribute(Attribute::Bold),
        Print(format!(" {} ", clip(title, inner.saturating_sub(2)))),
        SetAttribute(Attribute::Reset),
        ResetColor
    )?;

    let content_height = height.saturating_sub(2);
    let wrapped_lines = wrap_display_lines(lines, inner);
    for row in 0..content_height {
        let line = wrapped_lines.get(row as usize).cloned().unwrap_or_default();
        queue!(
            stdout,
            SetForegroundColor(color),
            MoveTo(x, y + 1 + row),
            Print("│"),
            ResetColor,
            Print(pad(&line, inner)),
            SetForegroundColor(color),
            Print("│"),
            ResetColor
        )?;
    }
    queue!(
        stdout,
        SetForegroundColor(color),
        MoveTo(x, y + height - 1),
        Print(format!("╰{}╯", "─".repeat(inner))),
        ResetColor
    )?;
    Ok(())
}

fn wrap_display_lines(lines: &[String], width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut wrapped = Vec::new();
    for line in lines {
        if line.is_empty() {
            wrapped.push(String::new());
            continue;
        }
        let mut remaining = line.as_str();
        while !remaining.is_empty() {
            let mut end = 0usize;
            let mut count = 0usize;
            for (byte_index, ch) in remaining.char_indices() {
                if count == width {
                    break;
                }
                end = byte_index + ch.len_utf8();
                count += 1;
            }
            if end == 0 {
                end = remaining.len();
            }
            wrapped.push(remaining[..end].to_string());
            remaining = remaining[end..].trim_start();
        }
    }
    wrapped
}

fn activity_lines(state: &ForgeTuiRuntimeState, max_lines: usize) -> Vec<String> {
    let start = state.activity.len().saturating_sub(max_lines.max(1));
    state.activity[start..].to_vec()
}

fn draw_input_lines(_report: &ForgeTuiReport, state: &ForgeTuiRuntimeState) -> Vec<String> {
    let prompt = prompt_label(state);
    let mut lines = vec![format!("{prompt} {}", state.input)];

    if state.shell_mode {
        lines.push("Shell mode: type a local command; Tab or Esc returns.".to_string());
        return lines;
    }

    if !state.autocomplete.is_empty() {
        lines.extend(render_autocomplete_selector(state));
    } else if is_mask_field(&state.input) {
        lines.extend(render_mask_hint(&state.input));
    } else if is_counter_field(&state.input) {
        lines.extend(render_counter_hint(&state.input));
    }

    lines
}

fn refresh_tui_autocomplete(state: &mut ForgeTuiRuntimeState, report: &ForgeTuiReport) {
    state.autocomplete = tui_autocomplete_suggestions(report, &state.input);
    if state.autocomplete.is_empty() {
        state.autocomplete_index = 0;
    } else if state.autocomplete_index >= state.autocomplete.len() {
        state.autocomplete_index = 0;
    }
}

fn render_autocomplete_selector(state: &ForgeTuiRuntimeState) -> Vec<String> {
    let selected = state
        .autocomplete_index
        .min(state.autocomplete.len().saturating_sub(1));
    let total = state.autocomplete.len();
    let max_visible = 4usize;
    let start = selected
        .saturating_sub(1)
        .min(total.saturating_sub(max_visible));
    let end = (start + max_visible).min(total);
    let mut lines = vec![format!(
        "Selector [{}/{}] Use ↑/↓ to inspect, Tab accepts",
        selected + 1,
        total
    )];
    lines.extend(
        state.autocomplete[start..end]
            .iter()
            .enumerate()
            .map(|(offset, suggestion)| {
                if start + offset == selected {
                    format!("> {}", suggestion)
                } else {
                    format!("  {}", suggestion)
                }
            }),
    );
    lines
}

fn move_autocomplete_selection(state: &mut ForgeTuiRuntimeState, delta: isize) {
    if state.autocomplete.is_empty() {
        state.autocomplete_index = 0;
        return;
    }
    let len = state.autocomplete.len() as isize;
    let next = (state.autocomplete_index as isize + delta).rem_euclid(len);
    state.autocomplete_index = next as usize;
}

fn accept_autocomplete_selection(state: &mut ForgeTuiRuntimeState) {
    let Some(selected) = state.autocomplete.get(state.autocomplete_index).cloned() else {
        return;
    };
    let prefix_len = state
        .input
        .split_whitespace()
        .last()
        .map(|token| token.len())
        .unwrap_or(0);
    if prefix_len == 0 {
        state.input = selected;
        return;
    }
    let keep_len = state.input.len().saturating_sub(prefix_len);
    let mut updated = state.input[..keep_len].trim_end().to_string();
    if !updated.is_empty() {
        updated.push(' ');
    }
    updated.push_str(&selected);
    state.input = updated;
    state.autocomplete.clear();
    state.autocomplete_index = 0;
}

fn is_counter_field(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    [
        "quantos", "quantas", "contador", "número", "numero", "count",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn is_mask_field(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    ["cpf", "telefone", "celular", "cep", "documento"]
        .iter()
        .any(|needle| lower.contains(needle))
}

fn render_counter_hint(input: &str) -> Vec<String> {
    let value = extract_counter_value(input).unwrap_or_default();
    vec![
        "Form mode: counter".to_string(),
        format!("Value: {} (↑/↓ adjusts)", value),
    ]
}

fn render_mask_hint(input: &str) -> Vec<String> {
    let value = extract_mask_value(input);
    let label = if input.to_ascii_lowercase().contains("cpf") {
        "CPF"
    } else if input.to_ascii_lowercase().contains("cep") {
        "CEP"
    } else {
        "masked field"
    };
    vec![
        format!("Form mode: {}", label),
        format!("Preview: {}", value),
    ]
}

fn adjust_counter_input(input: &mut String, delta: i64) {
    let value = extract_counter_value(input).unwrap_or_default();
    let next = (value + delta).max(0);
    *input = next.to_string();
}

fn extract_counter_value(input: &str) -> Option<i64> {
    let digits = input
        .chars()
        .filter(|ch| ch.is_ascii_digit() || *ch == '-')
        .collect::<String>();
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok()
}

fn extract_mask_value(input: &str) -> String {
    let digits = input
        .chars()
        .filter(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        return String::new();
    }
    if input.to_ascii_lowercase().contains("cpf") {
        return format_cpf(&digits);
    }
    if input.to_ascii_lowercase().contains("cep") {
        return format_cep(&digits);
    }
    format_phone_like(&digits)
}

fn format_cpf(digits: &str) -> String {
    let digits = digits.chars().take(11).collect::<String>();
    let mut out = String::new();
    for (index, ch) in digits.chars().enumerate() {
        if index == 3 || index == 6 {
            out.push('.');
        }
        if index == 9 {
            out.push('-');
        }
        out.push(ch);
    }
    out
}

fn format_cep(digits: &str) -> String {
    let digits = digits.chars().take(8).collect::<String>();
    if digits.len() <= 5 {
        return digits;
    }
    let (left, right) = digits.split_at(5);
    format!("{left}-{right}")
}

fn format_phone_like(digits: &str) -> String {
    let digits = digits.chars().take(11).collect::<String>();
    match digits.len() {
        0..=2 => digits,
        3..=6 => format!("({}) {}", &digits[..2], &digits[2..]),
        7..=10 => format!("({}) {}-{}", &digits[..2], &digits[2..6], &digits[6..]),
        _ => format!("({}) {}-{}", &digits[..2], &digits[2..7], &digits[7..]),
    }
}

fn collect_file_suggestions(project_root: Option<&Path>) -> Vec<String> {
    let root = match project_root {
        Some(root) => root,
        None => return Vec::new(),
    };

    let mut files = Vec::new();
    collect_file_suggestions_recursive(root, root, 0, 4, 64, &mut files);
    files
        .into_iter()
        .filter(|path| !path.starts_with("target/") && !path.starts_with(".git/"))
        .take(12)
        .collect()
}

fn collect_file_suggestions_recursive(
    root: &Path,
    current: &Path,
    depth: usize,
    max_depth: usize,
    max_files: usize,
    files: &mut Vec<String>,
) {
    if files.len() >= max_files || depth > max_depth {
        return;
    }

    let entries = match std::fs::read_dir(current) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    let mut paths = entries.filter_map(Result::ok).collect::<Vec<_>>();
    paths.sort_by(|left, right| left.file_name().cmp(&right.file_name()));

    for entry in paths {
        if files.len() >= max_files {
            break;
        }
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') && name != ".forge" {
            continue;
        }
        if path.is_dir() {
            collect_file_suggestions_recursive(root, &path, depth + 1, max_depth, max_files, files);
            continue;
        }
        if let Ok(relative) = path.strip_prefix(root) {
            if let Some(relative) = relative.to_str() {
                files.push(format!("@{relative}"));
            }
        }
    }
}

fn tui_autocomplete_suggestions(report: &ForgeTuiReport, input: &str) -> Vec<String> {
    let token = input.split_whitespace().last().unwrap_or("").trim();
    if token.is_empty() {
        return Vec::new();
    }
    let mut chars = token.chars();
    let prefix = chars.next().unwrap_or_default();
    let query = chars.as_str().trim().to_ascii_lowercase();
    let matches =
        |candidate: &str| query.is_empty() || candidate.to_ascii_lowercase().contains(&query);

    match prefix {
        '/' => {
            let mut suggestions = report
                .quick_commands
                .iter()
                .filter(|command| matches(command))
                .take(6)
                .cloned()
                .collect::<Vec<_>>();
            if suggestions.is_empty() {
                suggestions.push("/help".to_string());
            }
            suggestions
        }
        '@' => report
            .agent_suggestions
            .iter()
            .chain(report.file_suggestions.iter())
            .take(6)
            .cloned()
            .collect(),
        '~' => report
            .workflow_suggestions
            .iter()
            .take(6)
            .cloned()
            .collect(),
        '&' => report.context_suggestions.iter().take(6).cloned().collect(),
        _ => Vec::new(),
    }
}

fn touch_interaction(state: &mut ForgeTuiRuntimeState) {
    state.last_interaction_at = Instant::now();
}

fn set_shell_mode(state: &mut ForgeTuiRuntimeState, enabled: bool, reason: &str) {
    if state.shell_mode != enabled {
        state.shell_mode = enabled;
        state.autocomplete.clear();
        push_activity(
            state,
            if enabled {
                format!("Shell mode enabled ({reason})")
            } else {
                format!("Shell mode disabled ({reason})")
            },
        );
    }
    touch_interaction(state);
}

fn maybe_handoff_to_shell_due_to_idle(
    state: &mut ForgeTuiRuntimeState,
    now: Instant,
    timeout: Duration,
) -> bool {
    if state.shell_mode || state.last_interaction_at > now {
        return false;
    }
    if now.duration_since(state.last_interaction_at) < timeout {
        return false;
    }

    state.shell_mode = true;
    state.autocomplete.clear();
    push_activity(state, "Shell mode enabled after idle timeout".to_string());
    touch_interaction(state);
    true
}

fn push_activity(state: &mut ForgeTuiRuntimeState, line: String) {
    state.activity.push(line);
    if state.activity.len() > MAX_ACTIVITY_LINES {
        let remove_count = state.activity.len() - MAX_ACTIVITY_LINES;
        state.activity.drain(0..remove_count);
    }
}

fn prompt_label(state: &ForgeTuiRuntimeState) -> &'static str {
    if state.shell_mode {
        "forge-shell>"
    } else {
        "forge>"
    }
}

fn pad(text: &str, width: usize) -> String {
    let clipped = clip(text, width);
    let count = clipped.chars().count();
    if count >= width {
        clipped
    } else {
        format!("{clipped}{}", " ".repeat(width - count))
    }
}

fn clip(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let mut clipped = text.chars().take(width).collect::<String>();
    if text.chars().count() > width && width >= 1 {
        clipped.pop();
        clipped.push('…');
    }
    clipped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_report() -> ForgeTuiReport {
        ForgeTuiReport {
            schema_version: FORGE_TUI_SCHEMA_VERSION.to_string(),
            status: "forge_tui_ready".to_string(),
            layout: "opencode_style_orchestrator_first_tui".to_string(),
            orchestrator: ForgeTuiOrchestrator {
                schema_version: FORGE_TUI_ORCHESTRATOR_SCHEMA_VERSION.to_string(),
                default_interaction: "conversation".to_string(),
                decision_policy: "direct_answer_or_create_workflow".to_string(),
                plan_mode: "forge_workflow".to_string(),
                build_mode: "forge_workflow".to_string(),
                agent_model: "agents_and_subagents_are_workflows_or_nodes".to_string(),
                node_agent_routing: "per_node_agent_allowed".to_string(),
                summary: "summary".to_string(),
            },
            renderer_strategy: ForgeTuiRendererStrategy {
                schema_version: "forge.tui.renderer_strategy.v1".to_string(),
                current_backend: "rust_crossterm_fullscreen_fallback".to_string(),
                target_backend: "opentui_native_core_bridge_or_incremental_rust_port".to_string(),
                ecosystem_sources: vec![],
                rust_native_candidates: vec![],
                bridge_candidates: vec![],
                component_system_candidates: vec![],
                create_tui_template_family: "core".to_string(),
                next_step: "next".to_string(),
            },
            prompt: ForgeTuiPrompt {
                placeholder: "Talk to Forge; use /, @, ~, & for autocomplete".to_string(),
                submit_hint: "submit".to_string(),
                command_hint: "shell".to_string(),
            },
            shell: ForgeTuiShell {
                enabled: true,
                prefix: "!".to_string(),
                toggle: "!".to_string(),
                audit_event_kind: "forge_tui_shell_command".to_string(),
            },
            status_bar: ForgeTuiStatusBar {
                workflows: 3,
                active_runs: 1,
                events: 2,
                addons: 4,
                capabilities: 5,
                ready_handoffs: 1,
                pending_approvals: 0,
                estimated_cost_usd: 1.25,
            },
            visualizations: vec![],
            capabilities: vec![],
            quick_commands: vec![
                "/workflows".to_string(),
                "/agents".to_string(),
                "/approvals".to_string(),
            ],
            agent_suggestions: vec![
                "@codex — Codex".to_string(),
                "@opencode — OpenCode".to_string(),
                "@gemini — Gemini".to_string(),
            ],
            file_suggestions: vec![
                "@src/opencode_tui.rs".to_string(),
                "@tests/forge_cli_contract.rs".to_string(),
            ],
            workflow_suggestions: vec![
                "~workflow-1 — First workflow".to_string(),
                "~workflow-2 — Second workflow".to_string(),
            ],
            context_suggestions: vec![
                "&empresa.projeto.aplicacao".to_string(),
                "&empresa.projeto.web".to_string(),
            ],
        session_tabs: vec!["forge".to_string()],
        benchmark_snapshot: ForgeTuiBenchmarkSnapshot {
            schema_version: "forge.tui.benchmark_snapshot.v1".to_string(),
            summary: "summary".to_string(),
            placement_lines: vec![
                "Placement: Core = Codex, Gemini, OpenCode.".to_string(),
                "Placement: Addon-first = OpenClaw, Hermes, Open Design, Penpot, n8n.".to_string(),
            ],
            executor_lines: vec![
                "Codex: installed=true, configured=true, ready=true, path=/tmp/codex, command=codex".to_string(),
                "Gemini: installed=true, configured=true, ready=true, path=/tmp/gemini, command=gemini".to_string(),
                "OpenCode: installed=true, configured=true, ready=true, path=/tmp/opencode, command=opencode".to_string(),
                "Claude: installed=false, configured=false, ready=false, path=missing, command=claude".to_string(),
            ],
            forge_line: "Forge: workflows=3, active_runs=1, handoffs=1, approvals=0, costs=$1.2500".to_string(),
            live_notes: vec![],
        },
        notes: vec![],
    }
    }

    #[test]
    fn autocomplete_is_scoped_to_the_trigger_prefix() {
        let report = sample_report();

        assert_eq!(
            tui_autocomplete_suggestions(&report, "/wo"),
            vec!["/workflows".to_string()]
        );
        assert_eq!(
            tui_autocomplete_suggestions(&report, "@gem"),
            vec![
                "@codex — Codex".to_string(),
                "@opencode — OpenCode".to_string(),
                "@gemini — Gemini".to_string(),
                "@src/opencode_tui.rs".to_string(),
                "@tests/forge_cli_contract.rs".to_string()
            ]
        );
        assert_eq!(
            tui_autocomplete_suggestions(&report, "~wf"),
            vec![
                "~workflow-1 — First workflow".to_string(),
                "~workflow-2 — Second workflow".to_string()
            ]
        );
        assert_eq!(
            tui_autocomplete_suggestions(&report, "&emp"),
            vec![
                "&empresa.projeto.aplicacao".to_string(),
                "&empresa.projeto.web".to_string()
            ]
        );
        assert!(tui_autocomplete_suggestions(&report, "plain text").is_empty());
    }

    #[test]
    fn selector_and_form_hints_render_as_explicit_prompt_surfaces() {
        let report = sample_report();
        let state = ForgeTuiRuntimeState {
            project_root: None,
            chat_session_code: "chat_test".to_string(),
            chat_session_path: PathBuf::from("/tmp/chat_test.json"),
            chat_session_created_at: "2026-06-13T00:00:00Z".to_string(),
            input: "/wo".to_string(),
            shell_mode: false,
            activity: Vec::new(),
            autocomplete: tui_autocomplete_suggestions(&report, "/wo"),
            autocomplete_index: 0,
            conversation_history: Vec::new(),
            last_interaction_at: Instant::now(),
        };

        let selector = render_autocomplete_selector(&state);
        assert!(selector.first().unwrap().contains("Selector [1/1]"));
        assert!(selector.iter().any(|line| line.contains("> /workflows")));

        let mask_hint = render_mask_hint("CPF 12345678901");
        assert!(mask_hint[0].contains("Form mode"));
        assert!(mask_hint[1].contains("123.456.789-01"));

        let counter_hint = render_counter_hint("contador 7");
        assert!(counter_hint[0].contains("Form mode"));
        assert!(counter_hint[1].contains("Value: 7"));
    }

    #[test]
    fn wrap_display_lines_breaks_long_lines_for_panel_width() {
        let wrapped = wrap_display_lines(
            &[String::from("Placement: Addon-first = OpenClaw, Hermes, Open Design, Penpot, n8n.")],
            24,
        );

        assert!(wrapped.len() >= 3);
        assert!(wrapped[0].contains("Placement:"));
        assert!(wrapped.iter().any(|line| line.contains("OpenClaw")));
        assert!(wrapped.iter().any(|line| line.contains("Penpot")));
    }

    #[test]
    fn render_forge_tui_is_minimal_and_does_not_reintroduce_command_lists() {
        let report = sample_report();
        let rendered = render_forge_tui(&report);

        assert!(rendered.contains("Forge chat TUI - OpenCode-style"));
        assert!(rendered.contains("History and input stay on the live terminal surface."));
        assert!(!rendered.contains("Workflows:"));
        assert!(!rendered.contains("Agents:"));
        assert!(!rendered.contains("Subagents:"));
        assert!(!rendered.contains("Node agents:"));
    }

    #[test]
    fn idle_handoff_enables_shell_mode_after_timeout() {
        let mut state = ForgeTuiRuntimeState {
            project_root: None,
            chat_session_code: "chat_test".to_string(),
            chat_session_path: PathBuf::from("/tmp/chat_test.json"),
            chat_session_created_at: "2026-06-13T00:00:00Z".to_string(),
            input: String::new(),
            shell_mode: false,
            activity: Vec::new(),
            autocomplete: Vec::new(),
            autocomplete_index: 0,
            conversation_history: Vec::new(),
            last_interaction_at: Instant::now() - Duration::from_secs(10),
        };

        assert!(maybe_handoff_to_shell_due_to_idle(
            &mut state,
            Instant::now(),
            Duration::from_secs(5)
        ));
        assert!(state.shell_mode);
        assert!(state
            .activity
            .iter()
            .any(|line| line.contains("Shell mode enabled after idle timeout")));
    }

    #[test]
    fn autocomplete_selection_moves_and_can_be_accepted() {
        let mut state = ForgeTuiRuntimeState {
            project_root: None,
            chat_session_code: "chat_test".to_string(),
            chat_session_path: PathBuf::from("/tmp/chat_test.json"),
            chat_session_created_at: "2026-06-13T00:00:00Z".to_string(),
            input: "/wo".to_string(),
            shell_mode: false,
            activity: Vec::new(),
            autocomplete: vec!["/workflows".to_string(), "/workitems".to_string()],
            autocomplete_index: 0,
            conversation_history: Vec::new(),
            last_interaction_at: Instant::now(),
        };

        move_autocomplete_selection(&mut state, 1);
        assert_eq!(state.autocomplete_index, 1);
        accept_autocomplete_selection(&mut state);
        assert!(state.input.contains("/workitems"));
        assert!(state.autocomplete.is_empty());
    }

    #[test]
    fn conversation_history_is_transformed_for_brain_context() {
        let history = vec![
            ConversationTurn {
                role: ConversationRole::User,
                text: "Meu nome é Arthur".to_string(),
            },
            ConversationTurn {
                role: ConversationRole::Assistant,
                text: "Prazer".to_string(),
            },
        ];

        let context = conversation_context_for_brain(&history);
        assert_eq!(context.len(), 2);
        assert_eq!(context[0], "user: Meu nome é Arthur");
        assert_eq!(context[1], "assistant: Prazer");
    }

    #[test]
    fn chat_session_is_persisted_and_can_be_resumed_by_code() {
        let temp = tempfile::tempdir().unwrap();
        let project_root = temp.path();
        let store = ForgeStore::open(project_root.join("forge.sqlite")).unwrap();

        let session = create_chat_session_state(Some(project_root)).unwrap();
        let mut state = ForgeTuiRuntimeState {
            project_root: Some(project_root.to_path_buf()),
            chat_session_code: session.chat_session_code.clone(),
            chat_session_path: chat_session_path(Some(project_root), &session.chat_session_code),
            chat_session_created_at: session.created_at.clone(),
            input: String::new(),
            shell_mode: false,
            activity: Vec::new(),
            autocomplete: Vec::new(),
            autocomplete_index: 0,
            conversation_history: vec![
                ConversationTurn {
                    role: ConversationRole::User,
                    text: "Olá".to_string(),
                },
                ConversationTurn {
                    role: ConversationRole::Assistant,
                    text: "Oi".to_string(),
                },
            ],
            last_interaction_at: Instant::now(),
        };
        persist_chat_session(&state).unwrap();

        let loaded = load_chat_session_record(Some(project_root), Some(&session.chat_session_code))
            .unwrap()
            .unwrap();
        assert_eq!(loaded.chat_session_code, session.chat_session_code);
        assert_eq!(loaded.conversation_history.len(), 2);

        state.conversation_history.clear();
        let lines = resume_chat_session(
            &store,
            Some(project_root),
            &mut state,
            &format!("/resume {}", session.chat_session_code),
        )
        .unwrap();
        assert!(lines.iter().any(|line| line.contains("Resumed chat")));
        assert_eq!(state.conversation_history.len(), 2);
        assert_eq!(state.chat_session_code, session.chat_session_code);
    }
}
